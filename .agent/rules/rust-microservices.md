# Reglas de Microservicios Rust (`services/` y `crates/ikeda-core`)

## 1. Clean Architecture y Desacoplamiento
- Cada servicio en `services/<nombre>-api` sigue estrictamente las capas:
  - `domain/`: entidades, value objects, reglas de negocio y traits de repositorios/servicios (sin SQLx ni HTTP).
  - `application/`: casos de uso, comandos/DTOs y reglas RBAC (`rbac.rs`).
  - `infrastructure/`: implementaciones PostgreSQL/SQLx, storage (R2/local), clientes externos (Keycloak/QStash).
  - `presentation/`: handlers Axum, extractores, rutas OpenAPI (`utoipa` + Swagger UI en `/swagger-ui`).
- **Microservicios 100% agnósticos:** Prohibido realizar llamadas HTTP entre los 8 microservicios Rust. Toda orquestación multi-dominio pertenece a `portal/bff`.

## 2. Reutilización Obligatoria de `crates/ikeda-core`
- Usa siempre las primitivas de `ikeda-core` en lugar de reimplementarlas:
  - **Respuestas:** `ApiResponse<T>`, `PaginatedApiResponse<T>`, `PaginationMeta`.
  - **Errores:** `AppError` (sanitiza mensajes 500 automáticamente mediante `client_message()`).
  - **Consultas dinámicas:** `Criteria` + `ikeda_core::criteria::sqlx_builder` (features `["sqlx"]`), siempre con whitelist explícita de columnas y respetando límites DoS (`MAX_FILTER_DEPTH = 10`, `MAX_TOTAL_CONDITIONS = 50`, `MAX_IN_VALUES = 100`, `MAX_ORDER_BY_FIELDS = 5`).
  - **Observabilidad:** `init_tracing(service_name)`, `request_id_layer()`, `http_trace_layer(service_name)` (JSON plano compatible con Cloud Logging; nunca loggear `Authorization`, `X-Api-Key` ni `Cookie`).
  - **Auth/Tenant:** extractor `TenantContext` + `JwksJwtValidator` (RS256) + `CachedApiKeyValidator` (SHA-256, TTL 60s).
- La feature `dev-auth` (`ikeda-core/dev-auth`) debe permanecer desactivada por defecto en compilaciones de producción.

## 3. Calidad y Seguridad en Rust
- **Prohibido `unwrap()` y `expect()` fuera de `#[cfg(test)]`:** propaga errores con `?` hacia `AppError`.
- **Validación de API Keys Cross-DB:** Los 7 servicios de dominio (todos excepto `identity-api`) abren un pool secundario contra `IDENTITY_DATABASE_URL` (con fallback a `DATABASE_URL` en dev) usando `CachedApiKeyValidator` sobre la función SQL `SECURITY DEFINER` `public.authenticate_api_key(VARCHAR)`.
- **Gate obligatorio antes de cerrar cambios:**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
