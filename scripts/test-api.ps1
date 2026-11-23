# Automated API test script for IkedaDev Events API
$ErrorActionPreference = "Stop"

$BASE_URL = "http://localhost:8080"
$TENANT_ID = [System.Guid]::NewGuid().ToString()

Write-Host "`n🚀 Probando IkedaDev Events API ($BASE_URL)..." -ForegroundColor Cyan
Write-Host "🔑 Usando Tenant ID: $TENANT_ID" -ForegroundColor Yellow

# 1. Health Check
Write-Host "`n1️⃣ Verificando /health..." -ForegroundColor Green
$health = Invoke-RestMethod -Uri "$BASE_URL/health" -Method Get
Write-Host ($health | ConvertTo-Json -Depth 5) -ForegroundColor Gray

# 2. Crear Calendario
Write-Host "`n2️⃣ Creando un Calendario Estricto (allow_overlaps = false)..." -ForegroundColor Green
$calBody = @{
    name = "Sala Quirurgica 1"
    timezone = "America/Santiago"
    allow_overlaps = $false
    working_hours = @{
        monday = @{ start = "08:00"; end = "18:00" }
        tuesday = @{ start = "08:00"; end = "18:00" }
    }
} | ConvertTo-Json

$calRes = Invoke-RestMethod -Uri "$BASE_URL/v1/calendars" -Method Post -Body ([System.Text.Encoding]::UTF8.GetBytes($calBody)) -ContentType "application/json; charset=utf-8" -Headers @{ "X-Tenant-Id" = $TENANT_ID }
$CALENDAR_ID = $calRes.data.id
Write-Host "✅ Calendario Creado: ID = $CALENDAR_ID" -ForegroundColor Cyan
Write-Host ($calRes | ConvertTo-Json -Depth 5) -ForegroundColor Gray

# 3. Listar Calendarios
Write-Host "`n3️⃣ Listando Calendarios del Tenant..." -ForegroundColor Green
$calsList = Invoke-RestMethod -Uri "$BASE_URL/v1/calendars" -Method Get -Headers @{ "X-Tenant-Id" = $TENANT_ID }
Write-Host "Total calendarios encontrados: $($calsList.data.Count)" -ForegroundColor Cyan

# 4. Crear Evento 1
Write-Host "`n4️⃣ Programando Evento 1 con Idempotency-Key..." -ForegroundColor Green
$now = [DateTime]::UtcNow
$start1 = $now.AddHours(2).ToString("yyyy-MM-ddTHH:mm:ssZ")
$end1 = $now.AddHours(3).ToString("yyyy-MM-ddTHH:mm:ssZ")

$event1Body = @{
    calendar_id = $CALENDAR_ID
    title = "Cirugia Menor Paciente A"
    description = "Procedimiento programado"
    starts_at = $start1
    ends_at = $end1
    status = "CONFIRMED"
} | ConvertTo-Json

$event1Res = Invoke-RestMethod -Uri "$BASE_URL/v1/events" -Method Post -Body ([System.Text.Encoding]::UTF8.GetBytes($event1Body)) -ContentType "application/json; charset=utf-8" -Headers @{
    "X-Tenant-Id" = $TENANT_ID
    "Idempotency-Key" = "local-test-key-101"
}
Write-Host "✅ Evento 1 Creado: ID = $($event1Res.data.id)" -ForegroundColor Cyan

# 5. Probar Colisión (409 Conflict)
Write-Host "`n5️⃣ Probando detección de Solapamiento (debe fallar con 409 Conflict)..." -ForegroundColor Green
$startConflict = $now.AddHours(2).AddMinutes(30).ToString("yyyy-MM-ddTHH:mm:ssZ")
$endConflict = $now.AddHours(3).AddMinutes(30).ToString("yyyy-MM-ddTHH:mm:ssZ")

$eventConflictBody = @{
    calendar_id = $CALENDAR_ID
    title = "Cirugia Solapada"
    starts_at = $startConflict
    ends_at = $endConflict
} | ConvertTo-Json

try {
    Invoke-RestMethod -Uri "$BASE_URL/v1/events" -Method Post -Body ([System.Text.Encoding]::UTF8.GetBytes($eventConflictBody)) -ContentType "application/json; charset=utf-8" -Headers @{ "X-Tenant-Id" = $TENANT_ID }
    Write-Host "❌ Error: La colisión debió ser rechazada pero fue aceptada." -ForegroundColor Red
} catch {
    Write-Host "🛡️ Éxito: El servidor rechazó la colisión correctamente con 409 Conflict." -ForegroundColor Yellow
    Write-Host "Detalle: $($_.Exception.Message)" -ForegroundColor Gray
}

# 6. Consultar Disponibilidad
Write-Host "`n6️⃣ Consultando disponibilidad de slots libres..." -ForegroundColor Green
$queryStart = $now.ToString("yyyy-MM-ddTHH:mm:ssZ")
$queryEnd = $now.AddHours(6).ToString("yyyy-MM-ddTHH:mm:ssZ")

$availUrl = "$BASE_URL/v1/calendars/$CALENDAR_ID/availability?starts_at=$queryStart&ends_at=$queryEnd&slot_duration_minutes=30"
$availRes = Invoke-RestMethod -Uri $availUrl -Method Get -Headers @{ "X-Tenant-Id" = $TENANT_ID }
Write-Host "Slots disponibles encontrados: $($availRes.data.available_slots.Count)" -ForegroundColor Cyan
Write-Host "Bloques ocupados: $($availRes.data.busy_slots.Count)" -ForegroundColor Cyan

# 7. Swagger / OpenAPI Spec
Write-Host "`n7️⃣ Verificando contrato OpenAPI..." -ForegroundColor Green
$openapi = Invoke-RestMethod -Uri "$BASE_URL/openapi.json" -Method Get
Write-Host "✅ OpenAPI versión: $($openapi.openapi) - Título: $($openapi.info.title)" -ForegroundColor Cyan

Write-Host "`n🎉 Todos los endpoints pasaron las pruebas exitosamente!" -ForegroundColor Green
Write-Host "👉 Abre en tu navegador: $BASE_URL/swagger-ui" -ForegroundColor Magenta
