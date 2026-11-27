use super::state::AppState;
use crate::application::use_cases::{
    CreateClientCommand, CreateClientUseCase, DeleteClientUseCase, GetClientUseCase,
    SearchClientsUseCase, UpdateClientCommand, UpdateClientUseCase,
};
use crate::domain::models::Client;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use ikeda_core::response::{ApiResponse, PaginatedApiResponse};
use ikeda_core::tenant::TenantContext;
use uuid::Uuid;

fn require_crm_write(ctx: &TenantContext) -> Result<(), AppError> {
    if ctx.has_scope("crm:write")
        || ctx.has_scope("tenant:admin")
        || ctx.has_scope("platform:admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Missing required scope: 'crm:write', 'tenant:admin', or 'platform:admin'".to_string(),
        ))
    }
}

/// Creates a new client within the authenticated tenant.
#[utoipa::path(
    post,
    path = "/v1/clients",
    tag = "Clients",
    request_body = CreateClientCommand,
    responses(
        (status = 201, description = "Client created successfully", body = ApiResponse<Client>),
        (status = 409, description = "Conflict: duplicate email or tax_id in tenant", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid client payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn create_client(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(cmd): Json<CreateClientCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_crm_write(&tenant)?;
    let use_case = CreateClientUseCase::new(state.client_repo.clone());
    let client = use_case.execute(tenant.tenant_id, cmd).await?;
    let response = ApiResponse::created(client, "Client created successfully");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Retrieves an active client by ID within the authenticated tenant.
#[utoipa::path(
    get,
    path = "/v1/clients/{id}",
    tag = "Clients",
    params(
        ("id" = Uuid, Path, description = "Client unique identifier")
    ),
    responses(
        (status = 200, description = "Client found", body = ApiResponse<Client>),
        (status = 404, description = "Client not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn get_client(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let use_case = GetClientUseCase::new(state.client_repo.clone());
    let client = use_case.execute(tenant.tenant_id, id).await?;
    Ok(Json(ApiResponse::ok(client)))
}

/// Partially updates an existing client within the authenticated tenant.
#[utoipa::path(
    patch,
    path = "/v1/clients/{id}",
    tag = "Clients",
    params(
        ("id" = Uuid, Path, description = "Client unique identifier")
    ),
    request_body = UpdateClientCommand,
    responses(
        (status = 200, description = "Client updated successfully", body = ApiResponse<Client>),
        (status = 404, description = "Client not found", body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Conflict: duplicate email or tax_id in tenant", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Invalid update payload", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn patch_client(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateClientCommand>,
) -> Result<impl IntoResponse, AppError> {
    require_crm_write(&tenant)?;
    let use_case = UpdateClientUseCase::new(state.client_repo.clone());
    let updated = use_case.execute(tenant.tenant_id, id, cmd).await?;
    Ok(Json(ApiResponse::ok(updated)))
}

/// Soft-deletes a client by ID within the authenticated tenant.
#[utoipa::path(
    delete,
    path = "/v1/clients/{id}",
    tag = "Clients",
    params(
        ("id" = Uuid, Path, description = "Client unique identifier")
    ),
    responses(
        (status = 200, description = "Client deleted successfully", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Client not found", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn delete_client(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_crm_write(&tenant)?;
    let use_case = DeleteClientUseCase::new(state.client_repo.clone());
    use_case.execute(tenant.tenant_id, id).await?;
    Ok(Json(ApiResponse::message("Client deleted successfully")))
}

/// Searches clients using recursive `Criteria` filters (standard columns & JSONB `metadata.*`) with pagination.
#[utoipa::path(
    post,
    path = "/v1/clients/search",
    tag = "Clients",
    request_body = Criteria,
    responses(
        (status = 200, description = "Paginated clients matching criteria", body = PaginatedApiResponse<Client>),
        (status = 400, description = "Invalid search criteria", body = ApiResponse<serde_json::Value>),
        (status = 401, description = "Unauthorized tenant context", body = ApiResponse<serde_json::Value>)
    )
)]
pub async fn search_clients(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(criteria): Json<Criteria>,
) -> Result<impl IntoResponse, AppError> {
    let use_case = SearchClientsUseCase::new(state.client_repo.clone());
    let page = use_case.execute(tenant.tenant_id, criteria).await?;
    Ok(Json(page))
}
