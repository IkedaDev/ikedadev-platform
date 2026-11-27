use super::criteria_sql::{append_filter_group, append_order_by};
use crate::domain::models::{
    Article, ArticleRevision, ArticleSeo, ArticleStatus, ContentCategory, ContentTag,
};
use crate::domain::ports::{
    ArticleRepository, CreateArticleData, CreateRevisionData, UpdateArticleData,
};
use crate::infrastructure::database::{begin_tenant_tx, map_sqlx_error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct ArticleRow {
    id: Uuid,
    tenant_id: Uuid,
    category_id: Option<Uuid>,
    slug: String,
    title: String,
    excerpt: Option<String>,
    content_json: serde_json::Value,
    content_html: String,
    cover_image_url: Option<String>,
    status: String,
    published_at: Option<DateTime<Utc>>,
    scheduled_for: Option<DateTime<Utc>>,
    author_id: Option<Uuid>,
    author_name_snapshot: Option<String>,
    word_count: i32,
    reading_time_minutes: i32,
    seo: serde_json::Value,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct CategoryRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    slug: String,
    description: Option<String>,
    display_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<CategoryRow> for ContentCategory {
    fn from(r: CategoryRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            slug: r.slug,
            description: r.description,
            display_order: r.display_order,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct ArticleTagRow {
    article_id: Uuid,
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    slug: String,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct RevisionRow {
    id: Uuid,
    tenant_id: Uuid,
    article_id: Uuid,
    version_number: i32,
    title: String,
    content_json: serde_json::Value,
    content_html: String,
    edited_by: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl From<RevisionRow> for ArticleRevision {
    fn from(r: RevisionRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            article_id: r.article_id,
            version_number: r.version_number,
            title: r.title,
            content_json: r.content_json,
            content_html: r.content_html,
            edited_by: r.edited_by,
            created_at: r.created_at,
        }
    }
}

pub struct SqlxArticleRepository {
    pool: PgPool,
}

impl SqlxArticleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn hydrate_articles(
        tx: &mut Transaction<'_, Postgres>,
        tenant_id: Uuid,
        rows: Vec<ArticleRow>,
    ) -> Result<Vec<Article>, AppError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let article_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let category_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.category_id).collect();

        // Batch-load categories (no N+1)
        let mut categories_map: HashMap<Uuid, ContentCategory> = HashMap::new();
        if !category_ids.is_empty() {
            let cat_rows = sqlx::query_as::<_, CategoryRow>(
                r#"
                SELECT id, tenant_id, name, slug, description, display_order, created_at, updated_at, deleted_at
                FROM content_categories
                WHERE tenant_id = $1 AND deleted_at IS NULL AND id = ANY($2)
                "#,
            )
            .bind(tenant_id)
            .bind(&category_ids)
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to batch-load article categories"))?;

            for c in cat_rows {
                categories_map.insert(c.id, ContentCategory::from(c));
            }
        }

        // Batch-load tags (no N+1)
        let tag_rows = sqlx::query_as::<_, ArticleTagRow>(
            r#"
            SELECT at.article_id, t.id, t.tenant_id, t.name, t.slug, t.created_at, t.deleted_at
            FROM article_tags at
            INNER JOIN content_tags t
                ON t.id = at.tag_id AND t.tenant_id = at.tenant_id AND t.deleted_at IS NULL
            WHERE at.tenant_id = $1 AND at.article_id = ANY($2)
            ORDER BY t.name ASC
            "#,
        )
        .bind(tenant_id)
        .bind(&article_ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to batch-load article tags"))?;

        let mut tags_map: HashMap<Uuid, Vec<ContentTag>> = HashMap::new();
        for tr in tag_rows {
            tags_map.entry(tr.article_id).or_default().push(ContentTag {
                id: tr.id,
                tenant_id: tr.tenant_id,
                name: tr.name,
                slug: tr.slug,
                created_at: tr.created_at,
                deleted_at: tr.deleted_at,
            });
        }

        let mut articles = Vec::with_capacity(rows.len());
        for r in rows {
            let status = ArticleStatus::parse(&r.status)?;
            let seo: ArticleSeo = serde_json::from_value(r.seo).unwrap_or_default();
            let category = r
                .category_id
                .and_then(|cid| categories_map.get(&cid).cloned());
            let tags = tags_map.remove(&r.id).unwrap_or_default();

            articles.push(Article {
                id: r.id,
                tenant_id: r.tenant_id,
                category_id: r.category_id,
                category,
                tags,
                slug: r.slug,
                title: r.title,
                excerpt: r.excerpt,
                content_json: r.content_json,
                content_html: r.content_html,
                cover_image_url: r.cover_image_url,
                status,
                published_at: r.published_at,
                scheduled_for: r.scheduled_for,
                author_id: r.author_id,
                author_name_snapshot: r.author_name_snapshot,
                word_count: r.word_count,
                reading_time_minutes: r.reading_time_minutes,
                seo,
                metadata: r.metadata,
                created_at: r.created_at,
                updated_at: r.updated_at,
                deleted_at: r.deleted_at,
            });
        }

        Ok(articles)
    }

    async fn sync_article_tags(
        tx: &mut Transaction<'_, Postgres>,
        tenant_id: Uuid,
        article_id: Uuid,
        tag_ids: &[Uuid],
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM article_tags WHERE tenant_id = $1 AND article_id = $2")
            .bind(tenant_id)
            .bind(article_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to clear article tags"))?;

        for tag_id in tag_ids {
            sqlx::query(
                r#"
                INSERT INTO article_tags (tenant_id, article_id, tag_id)
                VALUES ($1, $2, $3)
                ON CONFLICT (tenant_id, article_id, tag_id) DO NOTHING
                "#,
            )
            .bind(tenant_id)
            .bind(article_id)
            .bind(tag_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to attach tag to article"))?;
        }
        Ok(())
    }
}

#[async_trait]
impl ArticleRepository for SqlxArticleRepository {
    async fn create(&self, data: CreateArticleData) -> Result<Article, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;
        let seo_value = serde_json::to_value(&data.seo)
            .map_err(|e| AppError::BadRequest(format!("Invalid SEO JSON: {e}")))?;

        let row = sqlx::query_as::<_, ArticleRow>(
            r#"
            INSERT INTO articles (
                tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                author_id, author_name_snapshot, word_count, reading_time_minutes,
                seo, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'DRAFT', $9, $10, $11, $12, $13, $14)
            RETURNING
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.category_id)
        .bind(&data.slug)
        .bind(&data.title)
        .bind(&data.excerpt)
        .bind(&data.content_json)
        .bind(&data.content_html)
        .bind(&data.cover_image_url)
        .bind(data.author_id)
        .bind(&data.author_name_snapshot)
        .bind(data.word_count)
        .bind(data.reading_time_minutes)
        .bind(&seo_value)
        .bind(&data.metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to create article"))?;

        if !data.tag_ids.is_empty() {
            Self::sync_article_tags(&mut tx, data.tenant_id, row.id, &data.tag_ids).await?;
        }

        let hydrated = Self::hydrate_articles(&mut tx, data.tenant_id, vec![row]).await?;
        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit article create: {e}"))
        })?;

        hydrated
            .into_iter()
            .next()
            .ok_or_else(|| AppError::InternalServerError("Hydrated article missing".to_string()))
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Article>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, ArticleRow>(
            r#"
            SELECT
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            FROM articles
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to find article by id"))?;

        let result = match row {
            Some(r) => Self::hydrate_articles(&mut tx, tenant_id, vec![r])
                .await?
                .into_iter()
                .next(),
            None => None,
        };
        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit find_by_id: {e}")))?;
        Ok(result)
    }

    async fn find_by_slug(&self, tenant_id: Uuid, slug: &str) -> Result<Option<Article>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, ArticleRow>(
            r#"
            SELECT
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            FROM articles
            WHERE slug = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(slug)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to find article by slug"))?;

        let result = match row {
            Some(r) => Self::hydrate_articles(&mut tx, tenant_id, vec![r])
                .await?
                .into_iter()
                .next(),
            None => None,
        };
        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit find_by_slug: {e}")))?;
        Ok(result)
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<(Vec<Article>, u64), AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let mut count_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT COUNT(*)::BIGINT FROM articles WHERE deleted_at IS NULL AND tenant_id = ",
        );
        count_qb.push_bind(tenant_id);
        if let Some(ref filters) = criteria.filter {
            count_qb.push(" AND ");
            append_filter_group(&mut count_qb, filters)?;
        }

        let total: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to count articles"))?;

        let mut data_qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            r#"
            SELECT
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            FROM articles
            WHERE deleted_at IS NULL AND tenant_id = "#,
        );
        data_qb.push_bind(tenant_id);
        if let Some(ref filters) = criteria.filter {
            data_qb.push(" AND ");
            append_filter_group(&mut data_qb, filters)?;
        }

        let order_by_slice = criteria.order_by.as_deref().unwrap_or(&[]);
        append_order_by(&mut data_qb, order_by_slice)?;

        let pagination = criteria.pagination.unwrap_or_default();
        let limit = i64::try_from(pagination.limit).unwrap_or(20);
        let offset = i64::try_from(pagination.offset()).unwrap_or(0);
        data_qb.push(" LIMIT ");
        data_qb.push_bind(limit);
        data_qb.push(" OFFSET ");
        data_qb.push_bind(offset);

        let rows = data_qb
            .build_query_as::<ArticleRow>()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| map_sqlx_error(e, "Failed to search articles"))?;

        let hydrated = Self::hydrate_articles(&mut tx, tenant_id, rows).await?;
        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit article search: {e}"))
        })?;

        Ok((hydrated, u64::try_from(total).unwrap_or(0)))
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateArticleData,
    ) -> Result<Article, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;

        let existing = sqlx::query_as::<_, ArticleRow>(
            r#"
            SELECT
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            FROM articles
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to load article for update"))?
        .ok_or_else(|| AppError::NotFound(format!("Article '{id}' not found")))?;

        let next_category_id = data.category_id.unwrap_or(existing.category_id);
        let next_slug = data.slug.unwrap_or(existing.slug);
        let next_title = data.title.unwrap_or(existing.title);
        let next_excerpt = data.excerpt.unwrap_or(existing.excerpt);
        let next_content_json = data.content_json.unwrap_or(existing.content_json);
        let next_content_html = data.content_html.unwrap_or(existing.content_html);
        let next_cover = data.cover_image_url.unwrap_or(existing.cover_image_url);
        let next_wc = data.word_count.unwrap_or(existing.word_count);
        let next_rt = data
            .reading_time_minutes
            .unwrap_or(existing.reading_time_minutes);
        let next_seo = match data.seo {
            Some(s) => serde_json::to_value(&s)
                .map_err(|e| AppError::BadRequest(format!("Invalid SEO JSON: {e}")))?,
            None => existing.seo,
        };
        let next_metadata = data.metadata.unwrap_or(existing.metadata);

        let updated_row = sqlx::query_as::<_, ArticleRow>(
            r#"
            UPDATE articles
            SET
                category_id = $3,
                slug = $4,
                title = $5,
                excerpt = $6,
                content_json = $7,
                content_html = $8,
                cover_image_url = $9,
                word_count = $10,
                reading_time_minutes = $11,
                seo = $12,
                metadata = $13,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(next_category_id)
        .bind(&next_slug)
        .bind(&next_title)
        .bind(&next_excerpt)
        .bind(&next_content_json)
        .bind(&next_content_html)
        .bind(&next_cover)
        .bind(next_wc)
        .bind(next_rt)
        .bind(&next_seo)
        .bind(&next_metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to update article"))?;

        if let Some(ref tag_ids) = data.tag_ids {
            Self::sync_article_tags(&mut tx, tenant_id, id, tag_ids).await?;
        }

        let hydrated = Self::hydrate_articles(&mut tx, tenant_id, vec![updated_row]).await?;
        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit article update: {e}"))
        })?;

        hydrated
            .into_iter()
            .next()
            .ok_or_else(|| AppError::InternalServerError("Hydrated article missing".to_string()))
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: ArticleStatus,
        published_at: Option<DateTime<Utc>>,
        scheduled_for: Option<DateTime<Utc>>,
    ) -> Result<Article, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, ArticleRow>(
            r#"
            UPDATE articles
            SET
                status = $3,
                published_at = $4,
                scheduled_for = $5,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(status.as_str())
        .bind(published_at)
        .bind(scheduled_for)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to update article status"))?
        .ok_or_else(|| AppError::NotFound(format!("Article '{id}' not found")))?;

        let hydrated = Self::hydrate_articles(&mut tx, tenant_id, vec![row]).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit status update: {e}")))?;

        hydrated
            .into_iter()
            .next()
            .ok_or_else(|| AppError::InternalServerError("Hydrated article missing".to_string()))
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<Article, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, ArticleRow>(
            r#"
            UPDATE articles
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING
                id, tenant_id, category_id, slug, title, excerpt,
                content_json, content_html, cover_image_url, status,
                published_at, scheduled_for, author_id, author_name_snapshot,
                word_count, reading_time_minutes, seo, metadata,
                created_at, updated_at, deleted_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to soft delete article"))?
        .ok_or_else(|| AppError::NotFound(format!("Article '{id}' not found")))?;

        let hydrated = Self::hydrate_articles(&mut tx, tenant_id, vec![row]).await?;
        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit article soft delete: {e}"))
        })?;

        hydrated
            .into_iter()
            .next()
            .ok_or_else(|| AppError::InternalServerError("Hydrated article missing".to_string()))
    }

    async fn create_revision(
        &self,
        data: CreateRevisionData,
        keep_last: i64,
    ) -> Result<ArticleRevision, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, data.tenant_id).await?;

        let next_version: i32 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(version_number), 0) + 1
            FROM article_revisions
            WHERE tenant_id = $1 AND article_id = $2
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.article_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to compute next revision version"))?;

        let row = sqlx::query_as::<_, RevisionRow>(
            r#"
            INSERT INTO article_revisions (
                tenant_id, article_id, version_number, title, content_json, content_html, edited_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, tenant_id, article_id, version_number, title, content_json, content_html, edited_by, created_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.article_id)
        .bind(next_version)
        .bind(&data.title)
        .bind(&data.content_json)
        .bind(&data.content_html)
        .bind(data.edited_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to insert article revision"))?;

        // Retain only the latest `keep_last` revisions
        sqlx::query(
            r#"
            DELETE FROM article_revisions
            WHERE tenant_id = $1 AND article_id = $2
              AND id NOT IN (
                  SELECT id FROM article_revisions
                  WHERE tenant_id = $1 AND article_id = $2
                  ORDER BY version_number DESC
                  LIMIT $3
              )
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.article_id)
        .bind(keep_last)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to prune old article revisions"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit article revision: {e}"))
        })?;

        Ok(ArticleRevision::from(row))
    }

    async fn list_revisions(
        &self,
        tenant_id: Uuid,
        article_id: Uuid,
    ) -> Result<Vec<ArticleRevision>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let rows = sqlx::query_as::<_, RevisionRow>(
            r#"
            SELECT id, tenant_id, article_id, version_number, title, content_json, content_html, edited_by, created_at
            FROM article_revisions
            WHERE tenant_id = $1 AND article_id = $2
            ORDER BY version_number DESC
            "#,
        )
        .bind(tenant_id)
        .bind(article_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to list article revisions"))?;

        tx.commit().await.map_err(|e| {
            AppError::DatabaseError(format!("Failed to commit list_revisions: {e}"))
        })?;

        Ok(rows.into_iter().map(ArticleRevision::from).collect())
    }

    async fn find_revision(
        &self,
        tenant_id: Uuid,
        article_id: Uuid,
        revision_id: Uuid,
    ) -> Result<Option<ArticleRevision>, AppError> {
        let mut tx = begin_tenant_tx(&self.pool, tenant_id).await?;
        let row = sqlx::query_as::<_, RevisionRow>(
            r#"
            SELECT id, tenant_id, article_id, version_number, title, content_json, content_html, edited_by, created_at
            FROM article_revisions
            WHERE tenant_id = $1 AND article_id = $2 AND id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(article_id)
        .bind(revision_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error(e, "Failed to find article revision"))?;

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(format!("Failed to commit find_revision: {e}")))?;

        Ok(row.map(ArticleRevision::from))
    }
}
