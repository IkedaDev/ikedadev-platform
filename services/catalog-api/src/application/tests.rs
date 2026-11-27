use super::use_cases::*;
use crate::domain::models::{
    BundleComponent, CatalogItem, CatalogItemVariant, Category, ItemDetails,
};
use crate::domain::repositories::{
    CatalogItemRepository, CatalogItemSearchPage, CategoryRepository,
};
use async_trait::async_trait;
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
struct MockCategoryRepo {
    categories: Mutex<Vec<Category>>,
}

#[async_trait]
impl CategoryRepository for MockCategoryRepo {
    async fn create(&self, _tenant_id: Uuid, category: &Category) -> Result<Category, AppError> {
        self.categories.lock().unwrap().push(category.clone());
        Ok(category.clone())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Category>, AppError> {
        Ok(self
            .categories
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.tenant_id == tenant_id && c.id == id && !c.is_deleted())
            .cloned())
    }

    async fn find_by_slug(
        &self,
        tenant_id: Uuid,
        slug: &str,
    ) -> Result<Option<Category>, AppError> {
        Ok(self
            .categories
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.tenant_id == tenant_id && c.slug == slug && !c.is_deleted())
            .cloned())
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Category>, AppError> {
        Ok(self
            .categories
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.tenant_id == tenant_id && !c.is_deleted())
            .cloned()
            .collect())
    }

    async fn update(&self, tenant_id: Uuid, category: &Category) -> Result<Category, AppError> {
        let mut lock = self.categories.lock().unwrap();
        if let Some(pos) = lock
            .iter()
            .position(|c| c.tenant_id == tenant_id && c.id == category.id && !c.is_deleted())
        {
            lock[pos] = category.clone();
            Ok(category.clone())
        } else {
            Err(AppError::NotFound("Category not found".into()))
        }
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.categories.lock().unwrap();
        if let Some(c) = lock
            .iter_mut()
            .find(|c| c.tenant_id == tenant_id && c.id == id && !c.is_deleted())
        {
            c.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Default)]
struct MockItemRepo {
    items: Mutex<Vec<CatalogItem>>,
    variants: Mutex<Vec<CatalogItemVariant>>,
}

#[async_trait]
impl CatalogItemRepository for MockItemRepo {
    async fn create(&self, _tenant_id: Uuid, item: &CatalogItem) -> Result<CatalogItem, AppError> {
        self.items.lock().unwrap().push(item.clone());
        Ok(item.clone())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<CatalogItem>, AppError> {
        let item = self
            .items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.tenant_id == tenant_id && i.id == id && !i.is_deleted())
            .cloned();
        Ok(item.map(|mut i| {
            i.variants = self
                .variants
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.tenant_id == tenant_id && v.item_id == id && !v.is_deleted())
                .cloned()
                .collect();
            i
        }))
    }

    async fn find_by_sku(
        &self,
        tenant_id: Uuid,
        sku: &str,
    ) -> Result<Option<CatalogItem>, AppError> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.tenant_id == tenant_id && i.sku.as_deref() == Some(sku) && !i.is_deleted())
            .cloned())
    }

    async fn find_by_ids_batch(
        &self,
        tenant_id: Uuid,
        ids: &[Uuid],
    ) -> Result<Vec<CatalogItem>, AppError> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.tenant_id == tenant_id && !i.is_deleted() && ids.contains(&i.id))
            .cloned()
            .collect())
    }

    async fn search_by_criteria(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<CatalogItemSearchPage, AppError> {
        let all: Vec<CatalogItem> = self
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.tenant_id == tenant_id && !i.is_deleted())
            .cloned()
            .collect();
        let total_items = all.len() as u64;
        let pagination = criteria.pagination.unwrap_or_default();
        let items = all
            .into_iter()
            .skip(pagination.offset() as usize)
            .take(pagination.sql_limit() as usize)
            .collect();
        Ok(CatalogItemSearchPage { items, total_items })
    }

    async fn update(&self, tenant_id: Uuid, item: &CatalogItem) -> Result<CatalogItem, AppError> {
        let mut lock = self.items.lock().unwrap();
        if let Some(pos) = lock
            .iter()
            .position(|i| i.tenant_id == tenant_id && i.id == item.id && !i.is_deleted())
        {
            lock[pos] = item.clone();
            Ok(item.clone())
        } else {
            Err(AppError::NotFound("Item not found".into()))
        }
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.items.lock().unwrap();
        if let Some(i) = lock
            .iter_mut()
            .find(|i| i.tenant_id == tenant_id && i.id == id && !i.is_deleted())
        {
            i.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn add_variant(
        &self,
        _tenant_id: Uuid,
        variant: &CatalogItemVariant,
    ) -> Result<CatalogItemVariant, AppError> {
        self.variants.lock().unwrap().push(variant.clone());
        Ok(variant.clone())
    }

    async fn find_variant_by_id(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<Option<CatalogItemVariant>, AppError> {
        Ok(self
            .variants
            .lock()
            .unwrap()
            .iter()
            .find(|v| {
                v.tenant_id == tenant_id
                    && v.item_id == item_id
                    && v.id == variant_id
                    && !v.is_deleted()
            })
            .cloned())
    }

    async fn update_variant(
        &self,
        tenant_id: Uuid,
        variant: &CatalogItemVariant,
    ) -> Result<CatalogItemVariant, AppError> {
        let mut lock = self.variants.lock().unwrap();
        if let Some(pos) = lock.iter().position(|v| {
            v.tenant_id == tenant_id
                && v.item_id == variant.item_id
                && v.id == variant.id
                && !v.is_deleted()
        }) {
            lock[pos] = variant.clone();
            Ok(variant.clone())
        } else {
            Err(AppError::NotFound("Variant not found".into()))
        }
    }

    async fn delete_variant(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        variant_id: Uuid,
    ) -> Result<bool, AppError> {
        let mut lock = self.variants.lock().unwrap();
        if let Some(v) = lock.iter_mut().find(|v| {
            v.tenant_id == tenant_id
                && v.item_id == item_id
                && v.id == variant_id
                && !v.is_deleted()
        }) {
            v.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[tokio::test]
async fn test_create_service_product_and_bundle_with_validations() {
    let cat_repo = Arc::new(MockCategoryRepo::default());
    let item_repo = Arc::new(MockItemRepo::default());
    let create_item = CreateCatalogItemUseCase::new(cat_repo, item_repo.clone());
    let tenant_id = Uuid::new_v4();

    // 1. Create SERVICE
    let svc = create_item
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: Some("SVC-MASAJE-60".to_string()),
                name: "Masaje Descontracturante".to_string(),
                description: Some("Sesión de 60 minutos".to_string()),
                image_url: Some("https://cdn.example.com/catalog/masaje.webp".to_string()),
                gallery_urls: vec!["https://cdn.example.com/catalog/masaje-2.webp".to_string()],
                base_price: 35_000,
                currency: Some("CLP".to_string()),
                tax_rate_bps: Some(1900),
                tax_included: Some(true),
                details: ItemDetails::Service {
                    duration_minutes: 60,
                    buffer_minutes: 10,
                },
                is_active: Some(true),
                metadata: Some(serde_json::json!({ "room_type": "spa" })),
            },
        )
        .await
        .unwrap();
    assert_eq!(svc.details.item_type_str(), "SERVICE");
    assert_eq!(
        svc.image_url.as_deref(),
        Some("https://cdn.example.com/catalog/masaje.webp")
    );
    assert_eq!(svc.gallery_urls.len(), 1);

    // 2. Create PRODUCT
    let prod = create_item
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: Some("PRD-ACEITE-250".to_string()),
                name: "Aceite Esencial Lavanda".to_string(),
                description: None,
                image_url: None,
                gallery_urls: vec![],
                base_price: 12_990,
                currency: Some("CLP".to_string()),
                tax_rate_bps: Some(1900),
                tax_included: Some(true),
                details: ItemDetails::Product {
                    unit_of_measure: Some("ML".to_string()),
                    track_inventory: true,
                },
                is_active: Some(true),
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(prod.details.item_type_str(), "PRODUCT");

    // 3. Create BUNDLE combining svc + prod
    let bundle = create_item
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: Some("BND-RELAX-01".to_string()),
                name: "Pack Relax Completo".to_string(),
                description: None,
                image_url: None,
                gallery_urls: vec![],
                base_price: 42_000,
                currency: Some("CLP".to_string()),
                tax_rate_bps: Some(1900),
                tax_included: Some(true),
                details: ItemDetails::Bundle {
                    items: vec![
                        BundleComponent {
                            item_id: svc.id,
                            quantity: 1,
                        },
                        BundleComponent {
                            item_id: prod.id,
                            quantity: 1,
                        },
                    ],
                },
                is_active: Some(true),
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(bundle.details.item_type_str(), "BUNDLE");

    // 4. Duplicate SKU in same tenant -> 409 Conflict
    let dup = create_item
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: Some("SVC-MASAJE-60".to_string()),
                name: "Otro Servicio".to_string(),
                description: None,
                image_url: None,
                gallery_urls: vec![],
                base_price: 20_000,
                currency: None,
                tax_rate_bps: None,
                tax_included: None,
                details: ItemDetails::Service {
                    duration_minutes: 30,
                    buffer_minutes: 0,
                },
                is_active: None,
                metadata: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(dup, AppError::Conflict(_)));

    // 5. Invalid SERVICE duration (0) -> 400 BadRequest
    let bad_dur = create_item
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: None,
                name: "Bad Service".to_string(),
                description: None,
                image_url: None,
                gallery_urls: vec![],
                base_price: 10_000,
                currency: None,
                tax_rate_bps: None,
                tax_included: None,
                details: ItemDetails::Service {
                    duration_minutes: 0,
                    buffer_minutes: 0,
                },
                is_active: None,
                metadata: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(bad_dur, AppError::BadRequest(_)));

    // 6. Negative base_price -> 400 BadRequest
    let bad_price = create_item
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: None,
                name: "Negative Price".to_string(),
                description: None,
                image_url: None,
                gallery_urls: vec![],
                base_price: -100,
                currency: None,
                tax_rate_bps: None,
                tax_included: None,
                details: ItemDetails::Product {
                    unit_of_measure: None,
                    track_inventory: false,
                },
                is_active: None,
                metadata: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(bad_price, AppError::BadRequest(_)));

    // 7. Batch lookup returns requested items & rejects > 100 IDs
    let batch_uc = GetCatalogItemsBatchUseCase::new(item_repo);
    let found = batch_uc
        .execute(tenant_id, vec![svc.id, prod.id])
        .await
        .unwrap();
    assert_eq!(found.len(), 2);

    let too_many: Vec<Uuid> = (0..=MAX_BATCH_IDS).map(|_| Uuid::new_v4()).collect();
    let batch_err = batch_uc.execute(tenant_id, too_many).await.unwrap_err();
    assert!(matches!(batch_err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn test_image_url_validation_gallery_limits_and_patch_clear() {
    use crate::domain::models::{validate_gallery_urls, validate_image_url, MAX_IMAGE_URL_LEN};

    // Valid HTTPS URL
    assert!(validate_image_url("https://cdn.kropy.cl/img.webp", "image_url", false).is_ok());

    // HTTP rejected when allow_http=false, allowed when allow_http=true
    assert!(matches!(
        validate_image_url(
            "http://localhost:8087/v1/media/local/asset.webp",
            "image_url",
            false
        ),
        Err(AppError::BadRequest(_))
    ));
    assert!(validate_image_url(
        "http://localhost:8087/v1/media/local/asset.webp",
        "image_url",
        true
    )
    .is_ok());

    // Unsafe schemes and protocol-relative URLs rejected even when allow_http=true
    for bad in [
        "javascript:alert(1)",
        "data:image/svg+xml,<svg onload=alert(1)>",
        "//evil.example/img.png",
        "ftp://files.example/img.png",
        "not-a-url",
    ] {
        assert!(
            matches!(
                validate_image_url(bad, "image_url", true),
                Err(AppError::BadRequest(_))
            ),
            "Expected BadRequest for '{bad}'"
        );
    }

    // Overlong URL (> 2048 chars) rejected
    let overlong = format!("https://cdn.kropy.cl/{}", "a".repeat(MAX_IMAGE_URL_LEN));
    assert!(matches!(
        validate_image_url(&overlong, "image_url", true),
        Err(AppError::BadRequest(_))
    ));

    // Gallery: max 8 allowed, 9 rejected, duplicates rejected
    let eight: Vec<String> = (1..=8)
        .map(|i| format!("https://cdn.kropy.cl/item-{i}.webp"))
        .collect();
    assert_eq!(validate_gallery_urls(&eight, false).unwrap().len(), 8);

    let mut nine = eight.clone();
    nine.push("https://cdn.kropy.cl/item-9.webp".to_string());
    assert!(matches!(
        validate_gallery_urls(&nine, false),
        Err(AppError::BadRequest(_))
    ));

    let dup_gallery = vec![
        "https://cdn.kropy.cl/item-1.webp".to_string(),
        "https://cdn.kropy.cl/item-1.webp".to_string(),
    ];
    assert!(matches!(
        validate_gallery_urls(&dup_gallery, false),
        Err(AppError::BadRequest(_))
    ));

    // UpdateCatalogItemUseCase: omitting image_url preserves it, explicit null clears it
    let cat_repo = Arc::new(MockCategoryRepo::default());
    let item_repo = Arc::new(MockItemRepo::default());
    let create_uc = CreateCatalogItemUseCase::new(cat_repo.clone(), item_repo.clone());
    let update_uc = UpdateCatalogItemUseCase::new(cat_repo, item_repo);
    let tenant_id = Uuid::new_v4();

    let created = create_uc
        .execute(
            tenant_id,
            CreateCatalogItemCommand {
                category_id: None,
                sku: None,
                name: "Shampoo Orgánico".to_string(),
                description: None,
                image_url: Some("https://cdn.kropy.cl/shampoo.webp".to_string()),
                gallery_urls: vec!["https://cdn.kropy.cl/shampoo-2.webp".to_string()],
                base_price: 9_990,
                currency: Some("CLP".to_string()),
                tax_rate_bps: None,
                tax_included: None,
                details: ItemDetails::Product {
                    unit_of_measure: Some("ML".to_string()),
                    track_inventory: true,
                },
                is_active: Some(true),
                metadata: None,
            },
        )
        .await
        .unwrap();

    // Omitting image_url in PATCH JSON leaves it untouched
    let cmd_keep: UpdateCatalogItemCommand =
        serde_json::from_str(r#"{"name": "Shampoo Orgánico 500ml"}"#).unwrap();
    let kept = update_uc
        .execute(tenant_id, created.id, cmd_keep)
        .await
        .unwrap();
    assert_eq!(
        kept.image_url.as_deref(),
        Some("https://cdn.kropy.cl/shampoo.webp")
    );
    assert_eq!(kept.gallery_urls.len(), 1);

    // Passing image_url: null clears image_url and gallery_urls: [] clears gallery
    let cmd_clear: UpdateCatalogItemCommand =
        serde_json::from_str(r#"{"image_url": null, "gallery_urls": []}"#).unwrap();
    let cleared = update_uc
        .execute(tenant_id, created.id, cmd_clear)
        .await
        .unwrap();
    assert_eq!(cleared.image_url, None);
    assert!(cleared.gallery_urls.is_empty());
}
