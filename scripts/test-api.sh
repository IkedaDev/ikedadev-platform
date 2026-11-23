#!/usr/bin/env bash
set -e

BASE_URL="http://localhost:8080"
TENANT_ID=$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen 2>/dev/null || echo "a1b2c3d4-e5f6-7890-abcd-ef1234567890")

echo -e "\n\033[1;36m🚀 Probando IkedaDev Events API ($BASE_URL)...\033[0m"
echo -e "\033[1;33m🔑 Usando Tenant ID: $TENANT_ID\033[0m"

# 1. Health
echo -e "\n\033[1;32m1️⃣ Verificando /health...\033[0m"
curl -s -f "$BASE_URL/health" | jq . || curl -s "$BASE_URL/health"

# 2. Crear Calendario
echo -e "\n\033[1;32m2️⃣ Creando un Calendario Estricto...\033[0m"
CAL_RES=$(curl -s -X POST "$BASE_URL/v1/calendars" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: $TENANT_ID" \
  -d '{
    "name": "Sala Quirúrgica 1",
    "timezone": "America/Santiago",
    "allow_overlaps": false
  }')
echo "$CAL_RES"
CAL_ID=$(echo "$CAL_RES" | grep -o '"id":"[^"]*' | head -n1 | cut -d'"' -f4)

# 3. Crear Evento
echo -e "\n\033[1;32m3️⃣ Creando Evento con Idempotency-Key...\033[0m"
curl -s -X POST "$BASE_URL/v1/events" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: $TENANT_ID" \
  -H "Idempotency-Key: local-key-001" \
  -d "{
    \"calendar_id\": \"$CAL_ID\",
    \"title\": \"Cirugía Menor Paciente A\",
    \"starts_at\": \"2026-12-01T10:00:00Z\",
    \"ends_at\": \"2026-12-01T11:00:00Z\"
  }"

# 4. Probar Solapamiento
echo -e "\n\033[1;32m4️⃣ Probando Solapamiento (esperando 409 Conflict)...\033[0m"
curl -s -w "\nHTTP Status: %{http_code}\n" -X POST "$BASE_URL/v1/events" \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: $TENANT_ID" \
  -d "{
    \"calendar_id\": \"$CAL_ID\",
    \"title\": \"Cirugía Solapada\",
    \"starts_at\": \"2026-12-01T10:30:00Z\",
    \"ends_at\": \"2026-12-01T11:30:00Z\"
  }"

echo -e "\n\033[1;35m👉 Swagger UI: $BASE_URL/swagger-ui\033[0m\n"
