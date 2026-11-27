use crate::application::rbac::{
    require_context_write_permission, require_media_delete, require_media_read, require_media_write,
};
use crate::domain::models::{MediaContext, TenantQuotaLimits};
use crate::domain::services::{
    build_safe_storage_key, compute_quota_usage, enforce_quota_limits, extension_for_allowed_mime,
    validate_file_name_and_extension,
};
use crate::infrastructure::{LocalDevObjectStorage, LocalMediaConfig};
use chrono::Utc;
use ikeda_core::error::AppError;
use ikeda_core::tenant::TenantContext;
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn test_strict_mime_and_extension_validation() {
    assert_eq!(extension_for_allowed_mime("image/webp").unwrap(), "webp");
    assert_eq!(extension_for_allowed_mime("image/jpeg").unwrap(), "jpg");
    assert_eq!(extension_for_allowed_mime("image/png").unwrap(), "png");
    assert_eq!(extension_for_allowed_mime("image/avif").unwrap(), "avif");

    for bad_mime in [
        "image/svg+xml",
        "text/html",
        "application/javascript",
        "application/x-executable",
        "application/octet-stream",
    ] {
        assert!(
            extension_for_allowed_mime(bad_mime).is_err(),
            "Expected MIME '{bad_mime}' to be rejected"
        );
    }

    assert!(validate_file_name_and_extension("photo.jpeg", "jpg").is_ok());
    assert!(validate_file_name_and_extension("banner.webp", "webp").is_ok());
    assert!(validate_file_name_and_extension("malware.exe", "png").is_err());
    assert!(validate_file_name_and_extension("xss.svg", "png").is_err());
    assert!(validate_file_name_and_extension("fake.jpg", "png").is_err());
}

#[test]
fn test_anti_path_traversal_storage_key_generation() {
    let tid = Uuid::new_v4();
    let aid = Uuid::new_v4();

    let safe_name = validate_file_name_and_extension("../../etc/passwd.png", "png").unwrap();
    assert_eq!(safe_name, "passwd.png");

    let safe_other = validate_file_name_and_extension("..\\other-tenant\\logo.png", "png").unwrap();
    assert_eq!(safe_other, "logo.png");

    let key = build_safe_storage_key(tid, MediaContext::Content, aid, "png");
    assert_eq!(key, format!("tenants/{tid}/content/{aid}.png"));
    assert!(!key.contains(".."));
}

#[test]
fn test_quota_calculations_and_enforcement() {
    let limits = TenantQuotaLimits {
        max_storage_bytes: 1_000_000,
        max_file_size_bytes: 500_000,
    };

    assert!(enforce_quota_limits(100_000, 200_000, limits).is_ok());
    assert!(matches!(
        enforce_quota_limits(600_000, 0, limits),
        Err(AppError::UnprocessableEntity { .. })
    ));
    assert!(matches!(
        enforce_quota_limits(300_000, 800_000, limits),
        Err(AppError::UnprocessableEntity { .. })
    ));

    let usage = compute_quota_usage(250_000, 3, limits);
    assert_eq!(usage.used_bytes, 250_000);
    assert_eq!(usage.available_bytes, 750_000);
    assert_eq!(usage.asset_count, 3);
    assert!((usage.usage_percent - 25.0).abs() < 0.01);
}

#[test]
fn test_rbac_matrix_and_branding_protection() {
    let tid = Uuid::new_v4();

    let staff = TenantContext::new(tid).with_scopes(vec![
        "tenant:member".to_string(),
        "media:read".to_string(),
        "media:write".to_string(),
    ]);
    assert!(require_media_read(&staff).is_ok());
    assert!(require_media_write(&staff).is_ok());
    assert!(require_context_write_permission(&staff, MediaContext::Content).is_ok());
    assert!(require_context_write_permission(&staff, MediaContext::Catalog).is_ok());
    assert!(matches!(
        require_context_write_permission(&staff, MediaContext::Branding),
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        require_media_delete(&staff),
        Err(AppError::Forbidden(_))
    ));

    let admin = TenantContext::new(tid).with_scopes(vec!["tenant:admin".to_string()]);
    assert!(require_media_read(&admin).is_ok());
    assert!(require_media_write(&admin).is_ok());
    assert!(require_context_write_permission(&admin, MediaContext::Branding).is_ok());
    assert!(require_media_delete(&admin).is_ok());
}

#[test]
fn test_local_dev_hmac_presign_verification() {
    let storage = LocalDevObjectStorage::new(LocalMediaConfig {
        base_url: "http://localhost:8087/v1/media/local".to_string(),
        storage_dir: PathBuf::from("/tmp/test-media"),
        signing_secret: "unit-test-secret".to_string(),
    });
    let key = "tenants/11111111-1111-4111-8111-111111111111/content/22222222-2222-4222-8222-222222222222.webp";
    let future_exp = Utc::now().timestamp() + 300;
    let sig = storage.sign_upload(key, future_exp).unwrap();

    assert!(storage
        .verify_upload_signature(key, future_exp, &sig)
        .is_ok());
    assert!(matches!(
        storage.verify_upload_signature(key, future_exp, "deadbeef"),
        Err(AppError::Unauthorized(_))
    ));
    assert!(matches!(
        storage.verify_upload_signature(key, Utc::now().timestamp() - 10, &sig),
        Err(AppError::Unauthorized(_))
    ));
}
