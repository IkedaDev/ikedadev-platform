use super::use_cases::*;
use crate::domain::models::{ApiKeyRecord, ApiKeySummary, Tenant, TenantUser};
use crate::domain::repositories::{ApiKeyRepository, TenantRepository, TenantUserRepository};
use crate::infrastructure::MockIdentityProvider;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use ikeda_core::error::AppError;
use ikeda_core::tenant::hash_api_key;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
struct MockTenantRepo {
    tenants: Mutex<Vec<Tenant>>,
}

#[async_trait]
impl TenantRepository for MockTenantRepo {
    async fn create(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
        self.tenants.lock().unwrap().push(tenant.clone());
        Ok(tenant.clone())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Tenant>, AppError> {
        Ok(self
            .tenants
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id && t.deleted_at.is_none())
            .cloned())
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Tenant>, AppError> {
        Ok(self
            .tenants
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.slug == slug && t.deleted_at.is_none())
            .cloned())
    }

    async fn update(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
        let mut lock = self.tenants.lock().unwrap();
        if let Some(pos) = lock.iter().position(|t| t.id == tenant.id) {
            lock[pos] = tenant.clone();
            Ok(tenant.clone())
        } else {
            Err(AppError::NotFound("Tenant not found".into()))
        }
    }
}

#[derive(Default)]
struct MockApiKeyRepo {
    keys: Mutex<Vec<ApiKeyRecord>>,
}

#[async_trait]
impl ApiKeyRepository for MockApiKeyRepo {
    async fn create(
        &self,
        _tenant_id: Uuid,
        record: &ApiKeyRecord,
    ) -> Result<ApiKeySummary, AppError> {
        self.keys.lock().unwrap().push(record.clone());
        Ok(record.clone().into())
    }

    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, AppError> {
        Ok(self
            .keys
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.tenant_id == tenant_id)
            .cloned()
            .map(Into::into)
            .collect())
    }

    async fn revoke(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.keys.lock().unwrap();
        if let Some(k) = lock
            .iter_mut()
            .find(|k| k.tenant_id == tenant_id && k.id == id && k.revoked_at.is_none())
        {
            k.is_active = false;
            k.revoked_at = Some(Utc::now());
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Default)]
struct MockTenantUserRepo {
    users: Mutex<Vec<TenantUser>>,
}

#[async_trait]
impl TenantUserRepository for MockTenantUserRepo {
    async fn create(&self, _tenant_id: Uuid, user: &TenantUser) -> Result<TenantUser, AppError> {
        self.users.lock().unwrap().push(user.clone());
        Ok(user.clone())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenantUser>, AppError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.tenant_id == tenant_id && u.id == id && !u.is_deleted())
            .cloned())
    }

    async fn find_by_tenant(&self, tenant_id: Uuid) -> Result<Vec<TenantUser>, AppError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.tenant_id == tenant_id && !u.is_deleted())
            .cloned()
            .collect())
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.users.lock().unwrap();
        if let Some(u) = lock
            .iter_mut()
            .find(|u| u.tenant_id == tenant_id && u.id == id && !u.is_deleted())
        {
            u.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[tokio::test]
async fn test_tenant_lifecycle_use_cases() {
    let repo = Arc::new(MockTenantRepo::default());
    let create_uc = CreateTenantUseCase::new(repo.clone());
    let get_uc = GetTenantUseCase::new(repo.clone());
    let update_uc = UpdateTenantUseCase::new(repo);

    let created = create_uc
        .execute(CreateTenantCommand {
            slug: "Anami-Studio".to_string(),
            name: "Anami Studio".to_string(),
            config: Some(serde_json::json!({ "plan": "pro" })),
        })
        .await
        .unwrap();
    assert_eq!(created.slug, "anami-studio");
    assert_eq!(created.status, "ACTIVE");

    // Duplicate slug -> 409 Conflict
    let dup = create_uc
        .execute(CreateTenantCommand {
            slug: "anami-studio".to_string(),
            name: "Duplicate".to_string(),
            config: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(dup, AppError::Conflict(_)));

    let fetched = get_uc.execute(created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);

    let updated = update_uc
        .execute(
            created.id,
            UpdateTenantCommand {
                name: Some("Anami Studio SPA".to_string()),
                status: Some("SUSPENDED".to_string()),
                config: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "Anami Studio SPA");
    assert_eq!(updated.status, "SUSPENDED");
}

#[tokio::test]
async fn test_api_key_mint_hash_and_revoke() {
    let repo = Arc::new(MockApiKeyRepo::default());
    let create_uc = CreateApiKeyUseCase::new(repo.clone());
    let list_uc = ListApiKeysUseCase::new(repo.clone());
    let revoke_uc = RevokeApiKeyUseCase::new(repo.clone());
    let tenant_id = Uuid::new_v4();

    let created = create_uc
        .execute(
            tenant_id,
            CreateApiKeyCommand {
                name: "BFF Key".to_string(),
                scopes: vec!["crm:read".to_string(), "events:write".to_string()],
                expires_at: Some(Utc::now() + Duration::days(30)),
            },
        )
        .await
        .unwrap();

    assert!(created.raw_key.starts_with("ik_live_"));
    assert_eq!(created.key.key_prefix, &created.raw_key[..16]);

    // Verify stored hash matches SHA-256 of raw_key
    {
        let stored = repo.keys.lock().unwrap();
        assert_eq!(stored[0].key_hash, hash_api_key(&created.raw_key));
    }

    let list = list_uc.execute(tenant_id).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, created.key.id);

    revoke_uc.execute(tenant_id, created.key.id).await.unwrap();
    let second_revoke = revoke_uc.execute(tenant_id, created.key.id).await;
    assert!(matches!(second_revoke, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn test_user_provisioning_and_keycloak_disable_on_delete() {
    let user_repo = Arc::new(MockTenantUserRepo::default());
    let idp = Arc::new(MockIdentityProvider::default());
    let create_uc = CreateTenantUserUseCase::new(user_repo.clone(), idp.clone());
    let list_uc = ListTenantUsersUseCase::new(user_repo.clone());
    let delete_uc = DeleteTenantUserUseCase::new(user_repo, idp.clone());
    let tenant_id = Uuid::new_v4();

    let user = create_uc
        .execute(
            tenant_id,
            CreateTenantUserCommand {
                email: "Admin@Anami.cl".to_string(),
                full_name: "Sebastián Ikeda".to_string(),
                temporary_password: None,
                roles: vec!["tenant:admin".to_string()],
                send_invite_email: true,
            },
        )
        .await
        .unwrap();

    assert_eq!(user.email, "admin@anami.cl");
    assert_eq!(idp.created_users.lock().unwrap().len(), 1);
    assert_eq!(idp.created_users.lock().unwrap()[0].1.tenant_id, tenant_id);

    let users = list_uc.execute(tenant_id).await.unwrap();
    assert_eq!(users.len(), 1);

    delete_uc.execute(tenant_id, user.id).await.unwrap();
    assert_eq!(idp.disabled_users.lock().unwrap().len(), 1);
    assert_eq!(idp.disabled_users.lock().unwrap()[0], user.keycloak_user_id);

    let users_after = list_uc.execute(tenant_id).await.unwrap();
    assert!(users_after.is_empty());
}
