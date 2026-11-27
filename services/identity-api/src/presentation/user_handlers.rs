use super::api_key_handlers::SCOPE_TENANT_ADMIN;
use super::state::AppState;
use super::tenant_handlers::SCOPE_PLATFORM_ADMIN;
use crate::application::use_cases::{
    CreateTenantUserCommand, CreateTenantUserUseCase, DeleteTenantUserUseCase,
    ListTenantUsersUseCase,
};
use crate::domain::models::TenantUser;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use ikeda_core::error::AppError;
use ikeda_core::response::ApiResponse;
use ikeda_core::tenant::TenantContext;
use uuid::Uuid;

/// Required scope for reading tenant users without full admin privileges.
pub const SCOPE_IDENTITY_READ: &str = "identity:read";

fn require_tenant_or_platform_admin(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope(SCOPE_TENANT_ADMIN) || ctx.has_scope(SCOPE_PLATFORM_ADMIN) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'tenant:admin' or 'platform:admin'".to_string(),
        ))
    }
}

fn require_identity_read_or_admin(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope(SCOPE_TENANT_ADMIN)
        || ctx.has_scope(SCOPE_PLATFORM_ADMIN)
        || ctx.has_scope(SCOPE_IDENTITY_READ)
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'identity:read', 'tenant:admin', or 'platform:admin'"
                .to_string(),
        ))
    }
}

fn is_valid_role_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
}

fn validate_and_authorize_roles(ctx: &TenantContext, roles: &[String]) -> Result<(), AppError> {
    let is_platform_admin = ctx.has_scope(SCOPE_PLATFORM_ADMIN);
    for role in roles {
        let trimmed = role.trim();
        if !is_platform_admin
            && (trimmed.eq_ignore_ascii_case(SCOPE_PLATFORM_ADMIN) || trimmed == "*")
        {
            return Err(AppError::Forbidden(
                "Cannot assign 'platform:admin' or wildcard roles without 'platform:admin' scope"
                    .to_string(),
            ));
        }
        if !is_valid_role_identifier(trimmed) {
            return Err(AppError::BadRequest(format!(
                "Invalid role identifier '{role}': must be 1..=64 ASCII characters matching [a-zA-Z0-9:_-]"
            )));
        }
    }
    Ok(())
}

/// Provisions a new user in Keycloak (with `tenant_id` attribute) and persists them in `tenant_users`.
#[utoipa::path(
    post,
    path = "/v1/users",
    tag = "Users",
    request_body = CreateTenantUserCommand,
    responses(
        (status = 201, description = "User provisioned in Keycloak and tenant_users", body = ApiResponse<TenantUser>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Conflict: email already exists", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid user payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_user(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(cmd): Json<CreateTenantUserCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    validate_and_authorize_roles(&ctx, &cmd.roles)?;
    let use_case = CreateTenantUserUseCase::new(state.user_repo.clone(), state.idp.clone());
    let user = use_case.execute(ctx.tenant_id, cmd).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::created(user, "User provisioned successfully")),
    ))
}

/// Lists all active users belonging to the authenticated tenant.
#[utoipa::path(
    get,
    path = "/v1/users",
    tag = "Users",
    responses(
        (status = 200, description = "List of active tenant users", body = ApiResponse<Vec<TenantUser>>),
        (status = 403, description = "Forbidden: requires identity:read, tenant:admin, or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    require_identity_read_or_admin(&ctx)?;
    let use_case = ListTenantUsersUseCase::new(state.user_repo.clone());
    let users = use_case.execute(ctx.tenant_id).await?;
    Ok(Json(ApiResponse::ok(users)))
}

/// Disables a user in Keycloak and performs a soft delete in `tenant_users`.
#[utoipa::path(
    delete,
    path = "/v1/users/{id}",
    tag = "Users",
    params(
        ("id" = Uuid, Path, description = "Internal tenant user unique identifier")
    ),
    responses(
        (status = 200, description = "User disabled in Keycloak and soft-deleted", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Forbidden: requires tenant:admin or platform:admin", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "User not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_user(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_tenant_or_platform_admin(&ctx)?;
    let use_case = DeleteTenantUserUseCase::new(state.user_repo.clone(), state.idp.clone());
    use_case.execute(ctx.tenant_id, id).await?;
    Ok(Json(ApiResponse::message(
        "User disabled and deleted successfully",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{ApiKeyRecord, ApiKeySummary, Tenant, TenantUser};
    use crate::domain::repositories::{ApiKeyRepository, TenantRepository, TenantUserRepository};
    use crate::infrastructure::MockIdentityProvider;
    use crate::presentation::routes::create_router;
    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use ikeda_core::tenant::{ApiKeyValidator, AuthenticatedKey, HEADER_API_KEY};
    use std::sync::Arc;
    use tower::ServiceExt;

    struct DummyTenantRepo;
    #[async_trait]
    impl TenantRepository for DummyTenantRepo {
        async fn create(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
            Ok(tenant.clone())
        }
        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Tenant>, AppError> {
            Ok(None)
        }
        async fn find_by_slug(&self, _slug: &str) -> Result<Option<Tenant>, AppError> {
            Ok(None)
        }
        async fn update(&self, tenant: &Tenant) -> Result<Tenant, AppError> {
            Ok(tenant.clone())
        }
    }

    struct DummyApiKeyRepo;
    #[async_trait]
    impl ApiKeyRepository for DummyApiKeyRepo {
        async fn create(
            &self,
            _tenant_id: Uuid,
            record: &ApiKeyRecord,
        ) -> Result<ApiKeySummary, AppError> {
            Ok(record.clone().into())
        }
        async fn find_by_tenant(&self, _tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, AppError> {
            Ok(Vec::new())
        }
        async fn revoke(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, AppError> {
            Ok(false)
        }
    }

    struct DummyUserRepo;
    #[async_trait]
    impl TenantUserRepository for DummyUserRepo {
        async fn create(
            &self,
            _tenant_id: Uuid,
            user: &TenantUser,
        ) -> Result<TenantUser, AppError> {
            Ok(user.clone())
        }
        async fn find_by_id(
            &self,
            _tenant_id: Uuid,
            _id: Uuid,
        ) -> Result<Option<TenantUser>, AppError> {
            Ok(None)
        }
        async fn find_by_tenant(&self, _tenant_id: Uuid) -> Result<Vec<TenantUser>, AppError> {
            Ok(Vec::new())
        }
        async fn soft_delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }
    }

    struct ScopedApiKeyValidator {
        tenant_id: Uuid,
        scopes: Vec<String>,
    }

    #[async_trait]
    impl ApiKeyValidator for ScopedApiKeyValidator {
        async fn validate_key_hash(
            &self,
            _key_hash: &str,
        ) -> Result<Option<AuthenticatedKey>, AppError> {
            Ok(Some(AuthenticatedKey {
                tenant_id: self.tenant_id,
                scopes: self.scopes.clone(),
            }))
        }
    }

    fn build_app_with_scopes(scopes: Vec<&str>) -> axum::Router {
        let state = AppState::new(
            Arc::new(DummyTenantRepo),
            Arc::new(DummyUserRepo),
            Arc::new(DummyApiKeyRepo),
            Arc::new(MockIdentityProvider::default()),
            Arc::new(ScopedApiKeyValidator {
                tenant_id: Uuid::new_v4(),
                scopes: scopes.into_iter().map(ToString::to_string).collect(),
            }),
        );
        create_router(state)
    }

    #[tokio::test]
    async fn test_rejects_percent_encoded_and_case_variant_platform_admin_escalation() {
        let app = build_app_with_scopes(vec!["tenant:admin"]);

        // 1. Percent-encoded bypass attempt -> 400 Bad Request
        let encoded_payload = serde_json::json!({
            "email": "attacker@demo.com",
            "full_name": "Attacker",
            "roles": ["platform%3Aadmin"]
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/users")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::from(encoded_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 2. Uppercase bypass attempt -> 403 Forbidden
        let upper_payload = serde_json::json!({
            "email": "attacker2@demo.com",
            "full_name": "Attacker 2",
            "roles": ["PLATFORM:ADMIN"]
        });
        let resp2 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/users")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::from(upper_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp2.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_list_users_requires_admin_or_identity_read_scope() {
        let member_app = build_app_with_scopes(vec!["tenant:member", "crm:write"]);
        let resp = member_app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/users")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let admin_app = build_app_with_scopes(vec!["tenant:admin"]);
        let ok_resp = admin_app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/users")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_oversized_body_returns_payload_too_large() {
        let app = build_app_with_scopes(vec!["tenant:admin"]);
        let huge = "a".repeat(300 * 1024);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/users")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HEADER_API_KEY, "k")
                    .body(Body::from(format!(
                        r#"{{"email":"a@b.com","full_name":"{huge}","roles":[]}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
