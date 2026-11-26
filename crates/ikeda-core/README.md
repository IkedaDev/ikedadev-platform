# ikeda-core

`ikeda-core` es la librería central y compartida por todos los microservicios y APIs del ecosistema `ikedadev-platform`. Provee tipos estandarizados, manejo de errores homogéneo, extracción de contexto multi-tenant, un árbol dinámico de filtrado tipo Criteria Pattern y observabilidad HTTP estructurada.

## Módulos y Capacidades

### 1. `response` (`ApiResponse<T>` y `PaginatedApiResponse<T>`)
- Envoltorio estándar para respuestas de éxito o error con timestamp UTC ISO-8601.
- `PaginatedApiResponse<T>` con `PaginationMeta` (`page`, `limit`, `total_items`, `total_pages`, `has_next_page`, `has_prev_page`).
- Compatible con `utoipa::ToSchema` para generación de especificaciones OpenAPI 3.0 / 3.1.
- Implementa `IntoResponse` de Axum para integración directa en handlers.

### 2. `error` (`AppError`)
- Manejo centralizado de errores basado en `thiserror`.
- Mapeo estricto a códigos HTTP:
  - `BadRequest(String)` -> `400 Bad Request`
  - `Unauthorized(String)` -> `401 Unauthorized`
  - `Forbidden(String)` -> `403 Forbidden`
  - `NotFound(String)` -> `404 Not Found`
  - `Conflict(String)` -> `409 Conflict`
  - `UnprocessableEntity { message, errors }` -> `422 Unprocessable Entity`
  - `InternalServerError(String)` y `DatabaseError(String)` -> `500 Internal Server Error` (ocultando detalles sensibles al cliente y registrando trazas vía `tracing::error!`).
- Implementa `IntoResponse` retornando la misma estructura de `ApiResponse`.

### 3. `tenant` (`TenantContext` y Extractor Axum)
- Representa el contexto de ejecución multi-inquilino (`tenant_id: Uuid`, `user_id: Option<Uuid>`, `scopes: Vec<String>`, `auth_source: AuthSource`).
- Extractor `FromRequestParts` de Axum:
  - Lee cabeceras de desarrollo / gateway: `X-Tenant-Id`, `X-User-Id`, `X-Scopes`, `X-Api-Key`.
  - Estructurado para interceptar tokens JWT Bearer vía JWKS en fases posteriores.

### 4. `criteria` (Criteria Pattern Dinámico)
- Estructuras serializables (`serde` + `utoipa`) para consultas complejas:
  - `FilterCondition`: `field`, `operator` (`eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `contains`, `in`, `isNull`, etc.), `value`.
  - `FilterGroup`: combinación recursiva mediante `and` y `or`.
  - `OrderBy`: especificación de ordenamiento (`field`, `direction: asc | desc`).
  - `Pagination`: paginación delimitada con cálculo de `offset()` y límites de seguridad (`MAX_LIMIT = 100`).

### 5. `observability` (Tracing Estructurado y Correlación HTTP)
- `init_tracing(service_name: &str)`: inicializa el suscriptor global de `tracing` de forma idempotente (`.try_init()`, sin pánico si ya está configurado) leyendo las siguientes variables de entorno:
  - `RUST_LOG`: filtro de niveles (por defecto `info,tower_http=info,sqlx=warn`).
  - `LOG_FORMAT`: formato de salida `json` o `pretty` (por defecto `json`). En modo `json` emite logs estructurados planos a `stdout` compatibles con **Grafana Loki** (LogQL `| json`) y **Google Cloud Logging**.
- `REQUEST_ID_HEADER`: constante `"x-request-id"` usada para correlación distribuida de extremo a extremo.
- `request_id_layer()`: combina `SetRequestIdLayer` y `PropagateRequestIdLayer` (`tower-http`) para propagar `X-Request-Id` si viene en la petición o generar un UUID si no existe, devolviéndolo siempre en la respuesta HTTP.
- `http_trace_layer(service_name: &'static str)`: capa Tower/Axum reutilizable que registra en cada petición/respuesta `service`, `request_id`, `method`, `path`, `status`, `latency_ms` y `tenant_id` (cuando está disponible vía `X-Tenant-Id` o `TenantContext`). Debe montarse con `request_id_layer()` como capa más externa para que `http_trace_layer` capture el `X-Request-Id` generado.
- **Seguridad en logs:** las cabeceras sensibles (`Authorization`, `X-Api-Key`, `Cookie`) **nunca** se registran en logs ni spans HTTP.

## Verificación de Calidad

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```
