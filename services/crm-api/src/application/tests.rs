use super::use_cases::*;
use crate::domain::models::Client;
use crate::domain::repositories::{ClientRepository, ClientSearchPage};
use async_trait::async_trait;
use ikeda_core::criteria::{Criteria, Pagination};
use ikeda_core::error::AppError;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
pub struct MockClientRepository {
    pub clients: Mutex<Vec<Client>>,
}

#[async_trait]
impl ClientRepository for MockClientRepository {
    async fn create(&self, _tenant_id: Uuid, client: &Client) -> Result<Client, AppError> {
        self.clients.lock().unwrap().push(client.clone());
        Ok(client.clone())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Client>, AppError> {
        let lock = self.clients.lock().unwrap();
        Ok(lock
            .iter()
            .find(|c| c.tenant_id == tenant_id && c.id == id && !c.is_deleted())
            .cloned())
    }

    async fn find_by_email(
        &self,
        tenant_id: Uuid,
        email: &str,
    ) -> Result<Option<Client>, AppError> {
        let lock = self.clients.lock().unwrap();
        Ok(lock
            .iter()
            .find(|c| {
                c.tenant_id == tenant_id && !c.is_deleted() && c.email.as_deref() == Some(email)
            })
            .cloned())
    }

    async fn find_by_tax_id(
        &self,
        tenant_id: Uuid,
        tax_id: &str,
    ) -> Result<Option<Client>, AppError> {
        let lock = self.clients.lock().unwrap();
        Ok(lock
            .iter()
            .find(|c| {
                c.tenant_id == tenant_id && !c.is_deleted() && c.tax_id.as_deref() == Some(tax_id)
            })
            .cloned())
    }

    async fn update(&self, tenant_id: Uuid, client: &Client) -> Result<Client, AppError> {
        let mut lock = self.clients.lock().unwrap();
        if let Some(pos) = lock
            .iter()
            .position(|c| c.tenant_id == tenant_id && c.id == client.id && !c.is_deleted())
        {
            lock[pos] = client.clone();
            Ok(client.clone())
        } else {
            Err(AppError::NotFound("Client not found".into()))
        }
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let mut lock = self.clients.lock().unwrap();
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

    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<ClientSearchPage, AppError> {
        let lock = self.clients.lock().unwrap();
        let all: Vec<Client> = lock
            .iter()
            .filter(|c| c.tenant_id == tenant_id && !c.is_deleted())
            .cloned()
            .collect();
        let total_items = all.len() as u64;
        let pagination = criteria.pagination.unwrap_or_default();
        let offset = pagination.offset() as usize;
        let limit = pagination.sql_limit() as usize;
        let items = all.into_iter().skip(offset).take(limit).collect();
        Ok(ClientSearchPage { items, total_items })
    }
}

#[tokio::test]
async fn test_create_client_and_per_tenant_uniqueness() {
    let repo = Arc::new(MockClientRepository::default());
    let use_case = CreateClientUseCase::new(repo);
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let cmd = CreateClientCommand {
        full_name: "Ana María Pérez".to_string(),
        email: Some("ANA@Example.com".to_string()),
        phone: Some("+56912345678".to_string()),
        tax_id: Some("12.345.678-9".to_string()),
        address: Some("Av. Providencia 123".to_string()),
        notes: None,
        keycloak_user_id: None,
        is_active: None,
        metadata: Some(serde_json::json!({ "tier": "vip" })),
    };

    let created = use_case.execute(tenant_a, cmd.clone()).await.unwrap();
    assert_eq!(created.email.as_deref(), Some("ana@example.com"));
    assert_eq!(created.tax_id.as_deref(), Some("12.345.678-9"));

    // Duplicate email in same tenant -> 409 Conflict
    let dup_email = CreateClientCommand {
        full_name: "Otro Cliente".to_string(),
        email: Some("ana@example.com".to_string()),
        tax_id: Some("99.999.999-K".to_string()),
        ..cmd.clone()
    };
    assert!(matches!(
        use_case.execute(tenant_a, dup_email).await.unwrap_err(),
        AppError::Conflict(_)
    ));

    // Duplicate tax_id in same tenant -> 409 Conflict
    let dup_tax = CreateClientCommand {
        full_name: "Otro Cliente 2".to_string(),
        email: Some("otro@example.com".to_string()),
        tax_id: Some("12.345.678-9".to_string()),
        ..cmd.clone()
    };
    assert!(matches!(
        use_case.execute(tenant_a, dup_tax).await.unwrap_err(),
        AppError::Conflict(_)
    ));

    // Same email and tax_id in Tenant B -> allowed!
    let created_b = use_case.execute(tenant_b, cmd).await;
    assert!(created_b.is_ok());
}

#[tokio::test]
async fn test_update_and_soft_delete_client() {
    let repo = Arc::new(MockClientRepository::default());
    let create_uc = CreateClientUseCase::new(repo.clone());
    let update_uc = UpdateClientUseCase::new(repo.clone());
    let delete_uc = DeleteClientUseCase::new(repo.clone());
    let get_uc = GetClientUseCase::new(repo);
    let tenant = Uuid::new_v4();

    let c1 = create_uc
        .execute(
            tenant,
            CreateClientCommand {
                full_name: "Cliente Uno".to_string(),
                email: Some("uno@example.com".to_string()),
                phone: None,
                tax_id: Some("11.111.111-1".to_string()),
                address: None,
                notes: None,
                keycloak_user_id: None,
                is_active: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

    let c2 = create_uc
        .execute(
            tenant,
            CreateClientCommand {
                full_name: "Cliente Dos".to_string(),
                email: Some("dos@example.com".to_string()),
                phone: None,
                tax_id: Some("22.222.222-2".to_string()),
                address: None,
                notes: None,
                keycloak_user_id: None,
                is_active: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

    // Self-update with same email succeeds
    let updated = update_uc
        .execute(
            tenant,
            c1.id,
            UpdateClientCommand {
                full_name: Some("Cliente Uno Editado".to_string()),
                email: Some("uno@example.com".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.full_name, "Cliente Uno Editado");

    // Updating c1 with c2's tax_id fails with 409 Conflict
    let err = update_uc
        .execute(
            tenant,
            c1.id,
            UpdateClientCommand {
                tax_id: Some(c2.tax_id.unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));

    // Soft delete c1
    delete_uc.execute(tenant, c1.id).await.unwrap();
    assert!(matches!(
        get_uc.execute(tenant, c1.id).await.unwrap_err(),
        AppError::NotFound(_)
    ));
    assert!(matches!(
        delete_uc.execute(tenant, c1.id).await.unwrap_err(),
        AppError::NotFound(_)
    ));

    // Re-creating client with c1's old tax_id after soft delete succeeds
    let recreated = create_uc
        .execute(
            tenant,
            CreateClientCommand {
                full_name: "Cliente Uno Nuevo".to_string(),
                email: Some("uno@example.com".to_string()),
                phone: None,
                tax_id: Some("11.111.111-1".to_string()),
                address: None,
                notes: None,
                keycloak_user_id: None,
                is_active: None,
                metadata: None,
            },
        )
        .await;
    assert!(recreated.is_ok());
}

#[tokio::test]
async fn test_search_clients_pagination() {
    let repo = Arc::new(MockClientRepository::default());
    let create_uc = CreateClientUseCase::new(repo.clone());
    let search_uc = SearchClientsUseCase::new(repo);
    let tenant = Uuid::new_v4();

    for i in 1..=5 {
        create_uc
            .execute(
                tenant,
                CreateClientCommand {
                    full_name: format!("Client {i}"),
                    email: Some(format!("c{i}@example.com")),
                    phone: None,
                    tax_id: None,
                    address: None,
                    notes: None,
                    keycloak_user_id: None,
                    is_active: None,
                    metadata: None,
                },
            )
            .await
            .unwrap();
    }

    let criteria = Criteria::new().with_pagination(Pagination::new(2, 2));
    let res = search_uc.execute(tenant, criteria).await.unwrap();

    assert_eq!(res.data.len(), 2);
    assert_eq!(res.pagination.total_items, 5);
    assert_eq!(res.pagination.total_pages, 3);
    assert!(res.pagination.has_next_page);
    assert!(res.pagination.has_prev_page);
}
